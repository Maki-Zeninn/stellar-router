import { Injectable, NotFoundException } from '@nestjs/common';
import { CreateStudentWishlistDto } from './dto/create-student-wishlist.dto';
import { UpdateStudentWishlistDto } from './dto/update-student-wishlist.dto';

@Injectable()
export class StudentWishlistService {
  private readonly items: Array<{ id: string } & CreateStudentWishlistDto> = [];

  findAll() {
    return this.items;
  }

  findOne(id: string) {
    const item = this.items.find((entry) => entry.id === id);
    if (!item) {
      throw new NotFoundException('StudentWishlist item not found');
    }
    return item;
  }

  create(payload: CreateStudentWishlistDto) {
    const created = { id: crypto.randomUUID(), ...payload };
    this.items.push(created);
    return created;
  }

  update(id: string, payload: UpdateStudentWishlistDto) {
    const item = this.findOne(id);
    Object.assign(item, payload);
    return item;
  }

  remove(id: string) {
    const index = this.items.findIndex((entry) => entry.id === id);
    if (index === -1) {
      throw new NotFoundException('StudentWishlist item not found');
    }
    this.items.splice(index, 1);
    return { id, deleted: true };
  }
}
